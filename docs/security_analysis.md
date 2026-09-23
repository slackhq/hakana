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
