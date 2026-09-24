<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(bool $b): void {
header('Content-Type: application/json');
if ($b) { $x = 1; } else { $x = 2; }
echo json_encode(vec[$x, source()]);
}
