# Security Analysis in Hakana

Hakana can attempt to find connections between user-controlled input (like `HH\global_get('GET')['name']`) and places that we don’t want unescaped user-controlled input to end up (like `echo "<h1>$name</h1>"` by looking at the ways that data flows through your application (via assignments, function/method calls and array/property access).

You can enable this mode by running `<hakana path> security-check`. When taint analysis is enabled, no other analysis is performed.

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
- `UserPII` — any data that contains Personally-Identifiable Information. This must be explicitly annotated in your application with Hakana attributes.
- `UserPassword` — any data that contains user secrets (e.g. hashed password fields). This must be explicitly annotated in your application with Hakana attributes.
- `SystemSecret` — any data that contains system secrets (e.g. API keys). This must be explicitly annotated in your application with Hakana attributes.

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
- `HtmlAttribute` - used for anywhere that emits arbitrary HTML attributes
- `HtmlAttributeUri` - used for anywhere that emits arbitrary URIs embedded in HTML code
- `Logging` - used for anywhere that logs arbitrary strings
- `Output` - used for anywhere that `echo`s arbitrary strings

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
    echo $user['email']; // this is an error
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