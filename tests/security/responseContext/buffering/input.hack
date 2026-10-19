<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
ob_start();
echo '<html>';
header('Content-Type: application/json');
echo source();
