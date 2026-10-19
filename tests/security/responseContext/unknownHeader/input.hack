<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(string $header): void {
header('Content-Type: application/json');
header($header);
echo source();
}
