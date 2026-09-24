<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret()[]: string { return ''; }
header('Content-Type: application/json');
echo json_encode(secret());
