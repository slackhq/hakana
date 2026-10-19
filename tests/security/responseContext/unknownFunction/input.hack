<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
function opaque(): void {}
header('Content-Type: application/json');
opaque();
echo source();
