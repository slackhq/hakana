<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function show(int $i): void {
switch ($i) { case 1: header('Content-Type: application/json');
break; default: header('Content-Type: text/html'); }
echo source();
}
