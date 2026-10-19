<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
header_register_callback(() ==> header('Content-Type: text/html'));
header('Content-Type: application/json');
echo source();
