<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
if (true) { header('Content-Type: application/json');
 } else { header('Content-Type: text/html'); }
echo source();
