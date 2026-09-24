<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
header('Content-Type: application/json');
try { header('Content-Type: text/html'); } finally { echo source(); }
