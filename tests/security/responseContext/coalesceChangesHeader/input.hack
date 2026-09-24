<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(?string $s): void {
header('Content-Type: application/json');
$s ?? header('Content-Type: text/html');
echo source();
}
