<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
header('Content-Type: application/json');
echo json_encode(input());
