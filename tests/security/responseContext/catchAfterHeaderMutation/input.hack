<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
header('Content-Type: application/json');
try { header('Content-Type: text/html'); throw new Exception(); } catch (Exception $e) { echo source(); }
