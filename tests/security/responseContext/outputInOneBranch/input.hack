<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(bool $b): void {
if ($b) { echo '<html>'; }
header('Content-Type: application/json');
echo source();
}
