<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show((function(): void) $f): void {
header('Content-Type: application/json');
$f();
echo source();
}
