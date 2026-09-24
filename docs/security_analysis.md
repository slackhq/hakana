# Security Analysis in Hakana

Hakana can attempt to find connections between user-controlled input (like `HH\global_get('_GET')['name']`) and places that we don’t want unescaped user-controlled input to end up (like `echo "<h1>$name</h1>"`) by looking at the ways that data flows through your application (via assignments, function/method calls and array/property access).

You can enable this mode by running `<hakana path> security-check`. When taint analysis is enabled, no other analysis is performed.

`--threads` controls both file analysis and forward graph traversal (default: 8).
The traversal expands bounded batches of each BFS frontier in parallel, then
merges results in frontier order using one global visited set. Worker scheduling
does not change retained witnesses, sink reporting, or the depth limit for a given
graph. Backward reachability pruning remains serial. `--threads 1` runs traversal
synchronously; WebAssembly also uses that path. A `TaintAnalysisIncomplete` warning
still means that `--max-depth` stopped the search before it exhausted reachable states.

Tainted input is anything that can be controlled, wholly or in part, by a user of your application. In taint analysis, tainted input is called a _taint source_.

Example sources:

 - `$_GET[‘id’]`
 - `$_POST['email']`
 - `$_COOKIE['token']`

 Taint analysis tracks how data flows from taint sources into _taint sinks_. Taint sinks are places you really don’t want untrusted data to end up.

Example sinks:

 - `<form action={$endpoint}>...</form>`
 - `$conn->query("select * from users where name='$name'")`

## Taint Sources

Hakana recognises a number of taint sources, defined in the [SourceType enum](https://github.com/slackhq/hakana/blob/8bf6931357a140cacfd7361cc0e9c08d6b5e9258/src/code_info/taint.rs#L7) class:

- `UriRequestHeader` - any data that comes from a given request‘s URI
- `NonUriRequestHeader` - any data that comes from the non-URI component of a request (like POST data)
- `RawUserData` - any data that comes from persistently-stored data (i.e. longer-lived than a single request) that can be controlled by a user. This must be explicitly annotated in your application with Hakana attributes
- `UserData` - any data that belongs to a specific user
- `UserEmail` - any user's email
- `UserPII` — any data that contains Personally-Identifiable Information. This must be explicitly annotated in your application with Hakana attributes.
- `UserPassword` — any data that contains user secrets (e.g. hashed password fields). This must be explicitly annotated in your application with Hakana attributes.
- `SystemSecret` — any data that contains system secrets (e.g. API keys). This must be explicitly annotated in your application with Hakana attributes.

`UriRequestHeader` and `NonUriRequestHeader` have the same injection policy.
POST bodies and cookies can reach HTML and URL sinks just as GET parameters can.
`HH\global_get` recognizes `_GET`, `_REQUEST`, `_POST`, `_COOKIE`, request-derived
fields of `_SERVER`, and client metadata fields of `_FILES`. Unknown global
names are conservatively modeled as request data. `file_get_contents('php://input')`
is also a request source.

Numeric values retain privacy, secret, and authorization-key provenance.
Numeric conversion can remove syntax-injection taints without making an ID
authorized or making a secret safe to log.

## Taint Sinks

Hakana recognises a range of taint sinks, defined in the [SinkType enum](https://github.com/slackhq/hakana/blob/c23bd6183a705a40a1cfe1952b603b97fae9f555/src/code_info/taint.rs#L30)

- `HtmlTag` - used for anywhere that emits arbitrary HTML
- `Sql` - used for anywhere that can execute arbitrary SQL strings
- `Shell` - used for anywhere that can execute arbitrary shell commands 
- `FileSystem` - used for any read/write to an arbitrary file path
- `RedirectUri` - used for anywhere that redirects to an arbitrary URI
- `Unserialize` - used for anywhere that unserializes arbitrary data
- `Cookie` - used for anywhere that saves arbitrary cookie information
- `CurlHeader` - used for anywhere that sends arbitrary header information in a Curl request
- `CurlUri` - used for anywhere that sends Curl requests to an arbitrary URI
- `HtmlAttribute` - unescaped HTML attribute syntax
- `HtmlAttributeUri` - navigation URLs, including links, base URLs and form destinations
- `HtmlActiveResourceUri` - script, iframe, object, embed and stylesheet URLs
- `HtmlMediaUri` - passive media/resource URLs; excluded from default request-injection taints
- `JavaScript` - script bodies and event-handler attributes
- `Css` - style bodies and style attributes
- `ResponseHeader` - the header argument of `header`
- `Logging` - used for anywhere that logs arbitrary strings
- `Output` - used for anywhere that `echo`s arbitrary strings
- `UnauthorizedDataFetchKey` - explicitly annotated data-access keys that require authorization

`curl_setopt` and literal `curl_setopt_array` options distinguish URLs/proxies
from HTTP headers and harmless options. Unknown options are treated
conservatively. Native exception messages, codes, and previous exceptions retain
their field-specific provenance when retrieved.

### XHP attribute contexts

Native XHP HTML elements escape scalar attribute values inside double quotes.
Those values do not need an `HtmlAttribute` injection check. Mixed/object values
retain that check because `UnsafeAttributeValue_DEPRECATED` can bypass escaping.
Custom component rendering does not acquire the native escaping guarantee.

Image `src`/`srcset`, media sources and video posters use `HtmlMediaUri`.
User control of a passive resource URL alone does not produce an injection report.
All attribute sinks retain `Output`, so secret disclosure is still reported.
The media label distinguishes this context for resource policies; the default
source policy does not enable a separate media-destination check.

Script, iframe, embed, object and stylesheet URLs use `HtmlActiveResourceUri`.
HTML escaping, URL-component encoding, numeric conversion and a fixed URL prefix
do not establish that the chosen resource is safe to execute. Existing
`Sanitize('HtmlAttributeUri')` annotations and URL checks do not clear the new
active-resource obligation. A reviewed resource builder can explicitly declare
`Sanitize('HtmlActiveResourceUri')`; `Sanitize('*')` includes it.

`link` classification uses this element's literal `rel`, `as` and `type`, regardless
of attribute order. Icons, metadata links and known passive preloads are passive;
stylesheet/module preloads, unknown relations, dynamic discriminators and spreads
remain conservative. Event handlers still use `JavaScript`, styles use `Css`, and
iframe `srcdoc` uses `HtmlTag` because the browser parses it as a nested document.

These are sink classifications, not sanitizers: displaying a value in an image
does not clear its taint before a later script, navigation or raw HTML use.

### Context-specific sanitization

HTML escaping removes HTML-tag taint, and quote escaping (`ENT_QUOTES`) also
removes HTML-attribute taint. It does not validate a URL scheme or escape
JavaScript/CSS. `strip_tags` only removes HTML-tag taint when no tags are allowed.
URL-component encoding is not a JavaScript or CSS sanitizer.

Concatenation only removes URL-destination taint from a suffix after a fixed
authority or an established relative path. For example,
`'https://example.com/path/' . $input` fixes the destination, whereas
`'https://' . $input` does not. Taint in the authority itself remains reportable.
That guarantee persists through subsequent operands in the same concatenation:
`'/apps/' . $id . '/' . $section . '?' . $query` cannot turn `$query` into a
URL scheme or host. This only removes destination-control taints (`HtmlAttributeUri`,
`CurlUri`, `RedirectUri`); executable-resource, HTML syntax and secret-disclosure
checks remain. Query parameter names and values should still be URL-encoded to
keep `&`, `=` and `#` inside their intended parameters.

### Search limits

The default maximum path depth is 40 and can be overridden by configuration or
`--max-depth` (1–255). Backward reachability from sinks prunes irrelevant parts
of the graph before the forward, context-sensitive traversal. If the forward
traversal reaches its depth limit with potentially relevant paths remaining,
`TaintAnalysisIncomplete` is emitted. This is an incomplete run, not evidence
that the code is free of security issues.

## Annotating your code for security analysis

Hakana understands a number of existing Hack sinks and sources — for example, it knows that the first argument of `AsyncMysqlConnection::query` is a `Sql` taint sink.

You may want to add more annotations — for example, annotations are necessary to describe `UserPII` sources.

Hakana comes with built-in support for [security analysis attributes](https://github.com/slackhq/hakana/tree/main/hack-lib/SecurityAnalysis) that allow to annotate your code appropriately.

### `Hakana\SecurityAnalysis\IgnorePath`

Use this attribute for a function or method that can never be executed in a production context.

### `Hakana\SecurityAnalysis\IgnorePathIfTrue`

Use this attribute for any function or method that determines whether the execution context is production or not.

```hack
<<Hakana\SecurityAnalysis\IgnorePathIfTrue()>>
function is_dev(): bool {
    ...
}

function foo(): void {
    if (is_dev()) {
        $a = $_GET['a'];
        echo $a; // this is fine
    }

    $a = $_GET['a'];
    echo $a; // this gets flagged
}
```

### `Hakana\SecurityAnalysis\RemoveTaintsWhenReturningTrue`

Use this attribute for any function or method that checks whether a value is "safe".

```hack
function is_valid_countrycode(
    <<Hakana\SecurityAnalysis\RemoveTaintsWhenReturningTrue('HtmlTag')>>
    string $country_code
): bool {
    ...
}

function foo(): void {
    $a = $_GET['a'];

    if (is_valid_countrycode($a)) {
        echo $a;
    }
}
```


### `Hakana\SecurityAnalysis\Sanitize`

Use this attribute for any function or method that sanitizes its input in a manner that Hakana cannot understand.

```hack
<<\Hakana\SecurityAnalysis\Sanitize('HtmlTag')>>
function custom_html_escape(string $arg): string {
    ...
}

$tainted = $_GET['foo'];
echo custom_html_escape($tainted);
```

On a parameter, `Sanitize` removes the named obligations from that argument's
path into the callee. It does not sanitize the caller's variable, other
arguments, or sources read within the callee. This is useful for a URL builder
whose path argument cannot change the configured origin. It is an asserted
contract, so review every use of that parameter before annotating it.

### `Hakana\SecurityAnalysis\NotSourceWhen`

Use this on a source function or method to exclude specific literal argument
values from the source declaration:

```hack
<<Hakana\SecurityAnalysis\Source('NonUriRequestHeader'),
  Hakana\SecurityAnalysis\NotSourceWhen('key', 'TRUSTED_PROXY_FIELD')>>
function request_field(string $key, string $default = ''): string {
    // ...
}
```

The first attribute argument names a parameter; subsequent arguments list exact,
case-sensitive string values. A call introduces no new source when every possible
literal value of that argument is listed. Unknown values, mixed trusted/untrusted
alternatives, and omitted arguments remain conservative. Invalid parameter names
have no effect.

This is a source contract, not a sanitizer: taint propagated from the function
body or other arguments, including a tainted default, remains. Wrappers around
request globals need a call-site boundary model to distinguish their fields.
Only assert this contract for a field whose origin is trusted in the deployment.

### `Hakana\SecurityAnalysis\HtmlSafeJson`

This describes a function or method returning a complete JSON expression encoded
for insertion into HTML script text, such as `json_encode($value, JSON_HEX_TAG)`.
It adds an encoding marker to the return data-flow path. It does not change the
Hack type or remove the value's source provenance.

Direct `json_encode` calls with a provable `JSON_HEX_TAG` flag also establish the
marker. Unknown flags remain conservative. A bare encoded JSON expression can
satisfy a JavaScript sink. Once composed into script text, it requires an audited
consumer contract as described below. Hakana does not parse JavaScript.

Encoding is tracked separately for each route, so an encoded branch cannot clear
an unencoded branch's taint. Decoding, string transformations, and container
fetches invalidate the encoding proof. The marker does not sanitize URL sinks or
secret output.

### `Hakana\SecurityAnalysis\AcceptsHtmlSafeJson`

This marks a function or method whose JavaScript sinks have been audited to
handle HTML-safe JSON as complete data values. It applies to native XHP script
bodies directly inside that function, and to its parameters explicitly marked
`Sink('JavaScript')`.

```hack
use type Facebook\XHP\HTML\script;

<<Hakana\SecurityAnalysis\AcceptsHtmlSafeJson>>
function render_boot_data(mixed $data): script {
    $json = json_encode($data, JSON_HEX_TAG) as string;
    return <script>{'window.boot_data = '.$json.';'}</script>;
}
```

The audit must establish that encoded values are complete expressions, never
inserted inside quotes, template strings, comments or regular expressions, and
never interpreted as code or HTML by consumers such as `eval` or `innerHTML`.
Values used as navigation destinations or executable resource URLs need separate
URL sinks or validation, even when their JSON insertion is safe.

The encoding proof survives concatenation, assignment, and helper returns.
Every tainted route still needs its own proof: raw siblings, raw alternative
branches, and transformations after encoding retain their reports. The attribute
only discharges the JavaScript obligation at the audited sink; it does not
sanitize the function's return value, its callees, event-handler attributes,
resource URLs, or secret output. Re-audit the contract when script handling
changes.

### HTTP response context

`BlockContext` carries the current response content type (`Unknown`, `Html`, or
`Json`) and whether observed output has committed the headers. A known literal
Content-Type updates that context. Ordinary assignments, pure computation, and
other named HTTP headers preserve it. A JSON response omits the `HtmlTag` sink
for scalar `echo`/`print` output; the value's taints and secret-output obligations
remain intact.

`if`/`else`, ternary, and short-circuit joins retain only facts guaranteed on the
continuing paths. Headers inside one conditional branch cannot sanitize output
on another branch or before the header. Unknown calls invalidate the MIME fact;
output-buffering operations and control-flow joins that may have emitted output
also forget header mutability. Loops without observed output preserve header
mutability, so a Content-Type set after the loop can establish a new MIME fact.

The model assumes headers are mutable at function entry and tracks output
observed in that function. It is not an interprocedural proof about output
previously sent by callers or hidden inside unknown callees.

`json_encode` and `json_encode_with_error` preserve the response context for all
input types, including objects and `mixed`. The model assumes serialization
callbacks do not alter HTTP response state. Effects from evaluating the encoder's
arguments still apply, as do subsequent explicit header changes.

### `Hakana\SecurityAnalysis\ShapeSource`

Given a type alias that defines a shape, you can use `ShapeSource` to define per-field source types.

```hack
<<\Hakana\SecurityAnalysis\ShapeSource(dict[
    'password' => 'UserPassword'
])>>
type user_t = shape(
    'id' => int,
    'username' => string,
    'password' => string,
);

function takesUser(user_t $user) {
    echo $user['username']; // this is ok
    echo $user['password']; // this is an error
}
```

### `Hakana\SecurityAnalysis\Sink`

Use this attribute on any function or method params that you want to be considered as sinks in Hakana. You can pass the name of the taint sink as a string.

```hack
function fetch(int $id): string {
    return db_query("SELECT * FROM table WHERE id=" . $id);
}

function db_query(
    <<\Hakana\SecurityAnalysis\Sink('Sql')>>
    string $sql
): string {
    ..
}

$value = $_GET["value"];
$result = fetch($value);
```

### `Hakana\SecurityAnalysis\Source`

Use this attribute on any function of method that you want to be considered a source in Hakana. You can pass the name of the taint source as a string.

```hack
class User {
    <<\Hakana\SecurityAnalysis\Source('UserPII')>>
    public function getEmail() : string {
        ...
    }
}

function takesUser(User $user): void {
    echo $user->getEmail(); // this is an error
}
```

### `HAKANA_SECURITY_IGNORE`

In addition to attributes, Hakana supports using the `HAKANA_SECURITY_IGNORE[<SinkType>]` doc comment to suppress individual paths. This can be used when you want to deliberately do something that would otherwise be considered dangerous.

The comment must immediately precede the affected call, argument, or assignment,
with only whitespace between them (an argument-list opening parenthesis is also
allowed). A comment before a call applies to that call's arguments. It does not
suppress later calls on the same line. For regex match-output propagation,
place it immediately before the pattern argument.

```hack
function check_endpoint(string $s): void {
    // make curl req to see if a given endpoint is valid
}

function explictly_follow_user_uri(): void {
    $endpoint = $_POST['endpoint'];

    check_endpoint(
        /* HAKANA_SECURITY_IGNORE[CurlUri] stops taint analysis through this path  */
        $endpoint
    );
}
```
