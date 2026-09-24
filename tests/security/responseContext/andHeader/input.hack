<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(bool $b): void {
$b && header('Content-Type: application/json');
echo source();
}
