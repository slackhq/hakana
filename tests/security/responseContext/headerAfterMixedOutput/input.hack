<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
header('Content-Type: application/json');
if (source() === 'emit') { echo '{}'; }
header('Content-Type: text/html');
echo source();
