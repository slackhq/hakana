<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
final class Value { public function __toString(): string { header('Content-Type: text/html'); return ''; } }
header('Content-Type: application/json');
echo new Value(); echo source();
