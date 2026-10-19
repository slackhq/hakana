<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
$json = json_encode(secret());
header('Content-Type: application/json');
echo $json;
