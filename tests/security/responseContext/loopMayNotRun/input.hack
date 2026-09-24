<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(bool $b): void {
while ($b) { header('Content-Type: application/json');
break; }
echo source();
}
