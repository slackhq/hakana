<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
header('Content-Type: application/json');
foreach (vec[1, 2] as $x) { echo source(); header('Content-Type: text/html'); }
