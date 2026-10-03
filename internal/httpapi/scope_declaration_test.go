package httpapi

import (
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"
)

// WHAT THIS FILE IS FOR, AND WHY IT EXISTS RATHER THAN A CLEANER CHECK.
//
// `account_scope.yml` at the repository root declares which of pantry's entry
// points reach another publisher's rows. It is checked by cafaye/core's
// `harness/account_scope_check.py`, and on this repository that checker cannot
// see a single one of pantry's routes:
//
//     $ cd cafaye/core && python3 harness/account_scope_check.py ../pantry
//     …
//     FAIL account-scope.stale: get-service: registeredAt points at a line that
//     does not register it: internal/api/api.gen.go:710
//
// That line does register it. `internal/api/api.gen.go:710` is
//
//     r.Get(options.BaseURL+"/v1/services/{name}", wrapper.GetService)
//
// and the chi recogniser requires a `/`-prefixed string literal as the FIRST
// term after `(`, so `oapi-codegen`'s output is the one shape of Go route
// registration this harness cannot read. `undeclared` is silent because there is
// nothing to find, `surface.minimum` cannot be set above zero because the count
// it floors is zero on a service with four routes, and the one row whose path
// ends in a parameter is reported `stale` — falsely — because the fallback asks
// the line to contain the string `{}`.
//
// So the enumeration `undeclared` exists to police has no enforcement here at
// all, and this file is the enforcement. `mountedRoutes` above walks the LIVE
// router with `chi.Walk`, which is a fact about what pantry serves rather than
// about how it is written; the document is read as text for the same reason it
// is read as text in `routes_test.go`; and `account_scope.yml` is read as text
// too. Nothing here parses YAML, for the reason `routes_test.go` gives: a
// dependency added to read a file the compiler and the generator already parse
// is a dependency that costs more than the assertion it buys.
//
// THIS IS NOT CORE'S CHECK REDONE WEAKERLY. Core decides three states per
// declared row from the source; this file decides one question core cannot —
// does the file this repository publishes still agree with the router this
// repository serves and the document this repository generates its client from.
// A service whose routes are hand-written gets the stronger check and does not
// need this one. A service whose routes are GENERATED needs this one, because
// the generator is what made its routes unreadable.

const scopeDeclaration = "../../account_scope.yml"

const generatedRouter = "../../internal/api/api.gen.go"

// A declared entry point, read out of the declaration by text. The fields are
// the ones the assertions below need, and they are named after the file's keys
// so a failure can be read against the YAML.
type scopeRow struct {
	id       string
	kind     string
	method   string
	path     string
	file     string
	line     int
	declared bool
}

// The six shapes, anchored to two and four spaces, which is what separates a
// key from its value. `registeredAt` is matched in its FLOW form because that is
// the form courier's file and this one both write it in; a declaration that
// spelled it as a nested mapping would stop being read, which is why
// `TestTheDeclarationIsWrittenInTheFormThisFileReads` exists rather than a
// tolerance.
var (
	scopeRowStart = regexp.MustCompile(`(?m)^  - id: ([a-z][a-z0-9-]*)$`)
	scopeKind     = regexp.MustCompile(`(?m)^    kind: ([a-z]+)$`)
	scopeMethod   = regexp.MustCompile(`(?m)^    method: ([A-Z]+)$`)
	scopePath     = regexp.MustCompile(`(?m)^    path: (\S+)$`)
	scopeAccount  = regexp.MustCompile(`(?m)^    accountScoped: (true|false)$`)
	scopeAt       = regexp.MustCompile(`(?m)^    registeredAt: \{ file: ([^,]+), line: (\d+) \}$`)
	scopeMin      = regexp.MustCompile(`(?m)^  minimum: (\d+)$`)
	scopeKey      = regexp.MustCompile(`(?m)^  accountKey: ([A-Za-z_][A-Za-z0-9_]*)$`)
	// `r.Get("…"` and, because that is what `oapi-codegen` actually emits,
	// `r.Get(options.BaseURL+"…"`. The optional leading term is the WHOLE POINT
	// of this pattern and the one place this file is stricter-reading than
	// core's chi recogniser, which anchors the string literal to the `(` and so
	// cannot see a generated registration at all. A copy of the same bug in the
	// check written to catch it is the failure mode this pattern exists to avoid.
	generatedRoute = regexp.MustCompile(
		`\.(Get|Post|Put|Patch|Delete|Head|Options)\(\s*(?:[A-Za-z_][A-Za-z0-9_.]*\s*\+\s*)?"([^"]*)"`)
)

func scopeRows(t *testing.T) []scopeRow {
	t.Helper()
	raw, err := os.ReadFile(scopeDeclaration)
	if err != nil {
		t.Fatalf("reading %s: %v — this repository publishes its account-scope declaration "+
			"there, and a service that cannot read it is a service whose boundary is a "+
			"comment somewhere else", scopeDeclaration, err)
	}
	text := string(raw)
	starts := scopeRowStart.FindAllStringSubmatchIndex(text, -1)
	if len(starts) == 0 {
		t.Fatalf("%s declares no entry points, or none in the shape this file reads; the "+
			"patterns in scope_declaration_test.go no longer match the declaration", scopeDeclaration)
	}
	rows := make([]scopeRow, 0, len(starts))
	for i, at := range starts {
		// To the start of the next row, or to the end of the file: a key in one
		// entry point must not be read as a key in the next one.
		end := len(text)
		if i+1 < len(starts) {
			end = starts[i+1][0]
		}
		block := text[at[0]:end]
		row := scopeRow{id: scopeRowStart.FindStringSubmatch(text[at[0]:])[1]}
		if m := scopeKind.FindStringSubmatch(block); m != nil {
			row.kind = m[1]
		}
		if m := scopeMethod.FindStringSubmatch(block); m != nil {
			row.method = m[1]
		}
		if m := scopePath.FindStringSubmatch(block); m != nil {
			row.path = m[1]
		}
		if m := scopeAccount.FindStringSubmatch(block); m != nil {
			row.declared = m[1] == "true"
		}
		if m := scopeAt.FindStringSubmatch(block); m != nil {
			row.file = m[1]
			row.line, _ = strconv.Atoi(m[2])
		}
		rows = append(rows, row)
	}
	return rows
}

// THE DIRECTION CORE CANNOT SEE. A row here that the router does not serve is a
// published claim about a route that does not exist, and a route the router
// serves that no row covers is exactly `account-scope.undeclared` — the finding
// the whole format exists to produce, which on pantry is structurally unable to
// fire because the recognisers read zero registrations out of the generated
// router. Both directions are one `t.Errorf` each and no cleverness.
func TestEveryDeclaredEntryPointIsMounted(t *testing.T) {
	mounted := map[string]bool{}
	for _, route := range mountedRoutes(t) {
		mounted[route] = true
	}
	for _, row := range scopeRows(t) {
		if row.kind != "http" {
			continue
		}
		route := row.method + " " + row.path
		if !mounted[route] {
			t.Errorf("%s declares %s, and the router does not serve it. A declaration row "+
				"with no route behind it is a boundary a reader trusts and nothing enforces.",
				row.id, route)
		}
	}
}

func TestEveryMountedRouteIsDeclared(t *testing.T) {
	declared := map[string]bool{}
	for _, row := range scopeRows(t) {
		if row.kind != "http" {
			continue
		}
		declared[row.method+" "+row.path] = true
	}
	var missing []string
	for _, route := range mountedRoutes(t) {
		if !declared[route] {
			missing = append(missing, route)
		}
	}
	sort.Strings(missing)
	for _, route := range missing {
		t.Errorf("the router serves %s and account_scope.yml does not declare it. This is "+
			"account-scope.undeclared, asserted here because core's recognisers cannot read "+
			"the generated router — see the header of this file.", route)
	}
}

// THE ASSERTION THAT REPLACES `account-scope.stale` HERE, and it is the one that
// goes wrong on pantry today.
//
// `registeredAt` is a claim about a line, and the two halves of it are decided
// separately because they fail differently: a row that names the wrong LINE is a
// stale pointer and the fix is a number, while a row that names the right line
// with the wrong METHOD or PATH is a row about a route that does not exist. Core
// collapses both into `same_route`, which cannot see the registration at all
// here; this reads the literal the generator wrote and compares it to the row.
//
// The method group is compared against `row.method` rather than ignored, so a
// refactor that turns a `GET` into a `POST` at the same path is caught even
// though the path still matches. It is compared through `httpVerb` and not
// through `strings.ToUpper`, because a router's method name and an HTTP method
// are different strings and the difference is worth being able to see: chi spells
// it `Get`, the declaration spells it `GET`, and the second half of that pair
// (`Options`/`OPTIONS`) is the one a reader would get wrong from `ToUpper`.
var httpVerb = map[string]string{
	"Get": "GET", "Post": "POST", "Put": "PUT", "Patch": "PATCH",
	"Delete": "DELETE", "Head": "HEAD", "Options": "OPTIONS",
}

func TestEveryRegisteredAtLineStillRegistersThatRoute(t *testing.T) {
	raw, err := os.ReadFile(generatedRouter)
	if err != nil {
		t.Fatalf("reading %s: %v — every row in %s names a line in it",
			generatedRouter, err, scopeDeclaration)
	}
	lines := strings.Split(string(raw), "\n")
	for _, row := range scopeRows(t) {
		if row.file == "" || row.line < 1 {
			t.Errorf("%s names no readable registeredAt; this file reads it in the flow "+
				"form `{ file: …, line: … }`", row.id)
			continue
		}
		if row.line > len(lines) {
			t.Errorf("%s: %s has %d line(s) and %s points at %d", row.id,
				generatedRouter, len(lines), row.file, row.line)
			continue
		}
		line := lines[row.line-1]
		found := false
		for _, m := range generatedRoute.FindAllStringSubmatch(line, -1) {
			if httpVerb[m[1]] == row.method && m[2] == row.path {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("%s: %s:%d does not register %s %s. The line reads: %s",
				row.id, row.file, row.line, row.method, row.path, strings.TrimSpace(line))
		}
	}
}

// `surface.accountKey` exists so a service does not have to spell its tenancy
// key twice. It is the one field of the declaration with no other consumer in
// this repository — core reads it for `account-key-lost` and
// `scope-key-guessed` — so nothing here would notice it rotting back to
// `account_id`, which `carries_key` would still match case- and
// separator-insensitively and which no column in this schema has ever been
// called. The migrations are the authority for the spelling, and they are read
// rather than trusted.
func TestTheDeclaredAccountKeyIsTheMigrationsSpelling(t *testing.T) {
	raw, err := os.ReadFile(scopeDeclaration)
	if err != nil {
		t.Fatalf("reading %s: %v", scopeDeclaration, err)
	}
	match := scopeKey.FindStringSubmatch(string(raw))
	if match == nil {
		t.Fatalf("%s declares no surface.accountKey, so the checker falls back to four "+
			"spellings it chose — account-scope.scope-key-guessed", scopeDeclaration)
	}
	migrations, err := os.ReadFile("../../migrations/00006_rls.sql")
	if err != nil {
		t.Fatalf("reading ../../migrations/00006_rls.sql: %v", err)
	}
	if !strings.Contains(string(migrations), match[1]) {
		t.Errorf("account_scope.yml declares surface.accountKey %q and the policies do not "+
			"use it. The declaration's key is the one the DATABASE keys on, so a key nothing "+
			"in the schema uses is a key that stopped meaning anything.", match[1])
	}
}

// The patterns above are a crude scan and this is what makes the crudeness safe:
// it fails loudly, naming itself, the day the declaration stops being written in
// the shape it reads. A test that silently matched nothing is how a declaration
// rots while reporting green, and this file's header is an argument against
// exactly that.
// THE HONEST ZERO, asserted rather than assumed, and it is the assertion that
// makes `account-scope.nothing-scoped` — the one warning core prints on this
// repository — a fact rather than a shrug.
//
// `account_scope.yml` declares no `accountScoped: true` row, and the reason is
// not that nobody looked: nothing in this service's Go code resolves a
// publisher. The resolver exists in SQL (`pantry.current_publisher_id()`,
// `migrations/00005_functions.sql:62`) and `pantry.begin_publisher/1` exists to
// set it (`:108`), and NEITHER is called from a Go file — the service takes the
// `pantry_public` role and no identity, which `internal/pantrydb/db.go:125` and
// `:129` are the evidence for.
//
// So if a caller of `begin_publisher` appeared in production Go, every row in the
// declaration would have to become `accountScoped: true` with an `accountFrom`
// naming it, and it is the kind of edit that lands as one line in one file
// during a phase-2 write path. This fails the day it appears.
//
// COMMENTS ARE STRIPPED FIRST, and not as tidiness. All six occurrences of the
// word in this repository's production Go are in `//` prose — `db.go` says
// "pantry.current_publisher_id() is NULL" in a comment explaining exactly why
// the GUC is unset, which is the opposite of a caller. A grep would fire on the
// explanation and stay silent on the call.
func TestNoProductionGoFileResolvesAPublisher(t *testing.T) {
	for _, path := range productionGoFiles(t) {
		body := goCode(sourceOf(t, path))
		for _, line := range strings.Split(body, "\n") {
			for _, want := range []string{"begin_publisher", "current_publisher_id", "set_config"} {
				if strings.Contains(line, want) {
					t.Errorf("%s calls %s in production code. That makes a publisher identity "+
						"reachable from a request, and every account_scope.yml row declared "+
						"`accountScoped: false` with `reason: public` would then be false: the "+
						"declaration must be rewritten, not the test.",
						path, want)
				}
			}
		}
	}
}

// Every non-test `.go` file under `internal/` and `cmd/`, as paths relative to
// the repository root so a failure names a file a reader can open. Both
// directories rather than one because a publisher could be resolved by the
// main package before the router is built, and a boundary asserted over the
// service's handlers is not a boundary.
func productionGoFiles(t *testing.T) []string {
	t.Helper()
	var found []string
	for _, root := range []string{"../../internal", "../../cmd"} {
		err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if entry.IsDir() || !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
				return nil
			}
			found = append(found, path)
			return nil
		})
		if err != nil {
			t.Fatalf("walking %s: %v", root, err)
		}
	}
	if len(found) == 0 {
		t.Fatalf("found no production Go files; the walk above is not looking where it says")
	}
	sort.Strings(found)
	return found
}

func sourceOf(t *testing.T, path string) string {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("reading %s: %v", path, err)
	}
	return string(raw)
}

// Go's line comments and block comments removed, with the newlines left in
// place so a line number still means what it meant. Not a tokenizer and it does
// not need to be: a `//` inside a string literal is inside a quoted span no
// pattern here matches, and the cost of being wrong is one extra or one missing
// hit on one line.
func goCode(text string) string {
	for {
		block := strings.Index(text, "/*")
		if block < 0 {
			break
		}
		closer := strings.Index(text[block+2:], "*/")
		if closer < 0 {
			text = text[:block] + strings.Repeat("\n", strings.Count(text[block:], "\n"))
			break
		}
		text = text[:block] + strings.Repeat("\n", strings.Count(text[block:block+2+closer+2], "\n")) +
			text[block+2+closer+2:]
	}
	lines := strings.Split(text, "\n")
	for i, line := range lines {
		if at := strings.Index(line, "//"); at >= 0 {
			lines[i] = line[:at]
		}
	}
	return strings.Join(lines, "\n")
}

func TestTheDeclarationIsWrittenInTheFormThisFileReads(t *testing.T) {
	rows := scopeRows(t)
	for _, row := range rows {
		if row.kind != "http" {
			t.Errorf("%s declares kind %q and this file reads only `http` rows with a method "+
				"and a path; add a kind here rather than let the row go unchecked", row.id, row.kind)
		}
		if row.method == "" || row.path == "" {
			t.Errorf("%s has no method/path in the shape this file reads", row.id)
		}
		if row.file == "" || row.line < 1 {
			t.Errorf("%s has no registeredAt in the shape this file reads — the flow form "+
				"`{ file: …, line: … }`, one line. A row this file cannot point at a line "+
				"with is a row no test in this repository checks.", row.id)
		}
	}
	raw, err := os.ReadFile(scopeDeclaration)
	if err != nil {
		t.Fatalf("reading %s: %v", scopeDeclaration, err)
	}
	text := string(raw)
	if scopeMin.FindStringSubmatch(text) == nil {
		t.Errorf("%s has no surface.minimum in the shape this file reads; pantry's is 0 "+
			"because the checker reads 0 registrations out of the generated router, and a "+
			"floor that is not stated is a floor nobody lowers on purpose", scopeDeclaration)
	}
	if !strings.Contains(text, "surface:") {
		t.Errorf("%s has no surface block", scopeDeclaration)
	}
}
