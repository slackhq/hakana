<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
header('Content-Type: application/json');
header('Access-Control-Allow-Origin: *');
header('X-Request-Id: known');
echo source();
