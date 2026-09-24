<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
final class Data { public static function value()[]: string { return source(); } }
header('Content-Type: application/json');
echo json_encode(Data::value());
