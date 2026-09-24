<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(bool $b): void {
header('Content-Type: application/json');
if ($b) { header('Content-Type: text/html'); return; }
echo source();
}
