package httpapi

// The problem and trace plumbing. Two files' worth of behaviour, one file,
// because the two are the same mechanism: every non-2xx response and the
// `X-Trace-Id` header that support starts from.

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"

	"github.com/cafaye/pantry/internal/api"
)

// problemTypeBase is where every problem's `type` lives. The document pins it:
// the type is a stable URI and the `code` is the slug under it, so a client can
// switch on one and print the other.
const problemTypeBase = "https://errors.cafaye.com/"

// titles are fixed per code, as the document requires: "may be reworded without
// a version bump", which is a licence to change the wording and not the slug.
var titles = map[api.ProblemCode]string{
	api.Internal:         "Internal error",
	api.MethodNotAllowed: "Method not allowed",
	api.NotFound:         "Not found",
	api.Unavailable:      "Service unavailable",
	api.ValidationFailed: "Bad request",
}

// problemContentType is NOT application/json, and the document says so on every
// non-2xx response. A client that switches on the content type — which is the
// whole reason RFC 9457 exists — reads "application/json" as a service that
// ignored the convention, and this repository's rule is that no service invents
// its own error body.
const problemContentType = "application/problem+json"

// writeJSON writes a success body. Every 2xx response in this service goes
// through here, so the encoding and the content type are decided once.
func writeJSON(w http.ResponseWriter, r *http.Request, status int, body any) {
	if w.Header().Get("Content-Type") == "" {
		w.Header().Set("Content-Type", "application/json")
	}
	w.Header().Set("X-Trace-Id", traceIDFrom(r.Context()))
	w.WriteHeader(status)
	// A marshalling failure here means the handler built a body the document
	// does not allow. The header is already written, so the only honest thing
	// left is to stop rather than emit a truncated body a client would parse.
	if err := json.NewEncoder(w).Encode(body); err != nil {
		panic(fmt.Sprintf("httpapi: %s %s: response did not encode: %v", r.Method, r.URL.Path, err))
	}
}

// writeProblem writes an RFC 9457 problem document with core's extensions.
//
// Every argument is explicit rather than derived, because the two halves a
// client switches on are the slug and the status, and a helper that derived the
// status from the slug would make `not_found` reachable at a status nobody
// documented.
func writeProblem(w http.ResponseWriter, r *http.Request, code api.ProblemCode, status int, detail string) {
	title, ok := titles[code]
	if !ok {
		// An undeclared code is a defect in this repository, and the response
		// must still be a document the document describes.
		title, code = titles[api.Internal], api.Internal
		status = http.StatusInternalServerError
	}
	w.Header().Set("Content-Type", problemContentType)
	writeJSON(w, r, status, api.Problem{
		Type:     problemTypeBase + string(code),
		Title:    title,
		Status:   status,
		Detail:   detail,
		Instance: r.URL.Path,
		Code:     code,
		TraceID:  traceIDFrom(r.Context()),
	})
}

// paramDetail turns oapi-codegen's parameter-binding errors into one sentence
// naming the parameter and what was wrong with it.
//
// The generated errors are five types rather than one with an error code, each
// carrying the parameter name, so the honest answer to "which parameter" is five
// type switches. They are asserted by TestTheParameterErrorsNameTheirParameter,
// because the alternative — `err.Error()` — is oapi-codegen's phrasing in a
// client's face and does change between generator releases.
func paramDetail(err error) string {
	var (
		cookieErr *api.UnescapedCookieParamError
		unmarshal *api.UnmarshalingParamError
		required  *api.RequiredParamError
		format    *api.InvalidParamFormatError
		tooMany   *api.TooManyValuesForParamError
	)
	switch {
	case errors.As(err, &cookieErr):
		return "query parameter " + cookieErr.ParamName + " could not be read: " + err.Error()
	case errors.As(err, &unmarshal):
		return "query parameter " + unmarshal.ParamName + " could not be decoded: " + err.Error()
	case errors.As(err, &required):
		return "query parameter " + required.ParamName + " is required"
	case errors.As(err, &format):
		return "query parameter " + format.ParamName + " is not the shape the document declares"
	case errors.As(err, &tooMany):
		return "query parameter " + tooMany.ParamName + " was given more than one value"
	default:
		return err.Error()
	}
}

// traceMiddleware mints or adopts a trace id for every request and puts it in
// the context, so the problem writer and the logger quote the same id the
// header carries.
//
// An inbound `X-Trace-Id` is ADOPTED rather than replaced, which is what makes
// the id useful across a hop: guard can hand one down and support can search
// for it in every service that answered. It is echoed as a header on every
// response, not only on problems, because the document declares it on every
// response including the 200s.
func traceMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		id := r.Header.Get("X-Trace-Id")
		if !validTraceID(id) {
			id = mintTraceID()
		}
		w.Header().Set("X-Trace-Id", id)
		next.ServeHTTP(w, r.WithContext(contextWithTraceID(r.Context(), id)))
	})
}

// validTraceID accepts an inbound id only if it looks like the 16-byte hex this
// service mints. An id that came from the internet and is unbounded is a log
// injection vector and a header-size problem, so anything else is replaced
// rather than trusted.
func validTraceID(id string) bool {
	if len(id) != 32 {
		return false
	}
	_, err := hex.DecodeString(id)
	return err == nil
}

func mintTraceID() string {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		// crypto/rand does not fail in any way this service can recover from,
		// and an id that is not unique costs support its search. Panic rather
		// than serve requests whose trace ids may collide.
		panic("httpapi: crypto/rand is unavailable: " + err.Error())
	}
	return hex.EncodeToString(b[:])
}
