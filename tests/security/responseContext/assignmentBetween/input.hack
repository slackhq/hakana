<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
header('Content-Type: application/json');
$value = source();
$json = json_encode(dict['value' => $value]);
echo $json;
