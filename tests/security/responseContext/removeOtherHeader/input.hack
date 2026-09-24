<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
header('Content-Type: application/json');
header_remove('X-Request-Id');
echo source();
