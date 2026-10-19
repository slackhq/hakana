<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function source()[]: string { return ''; }
final class Value { public function __toString(): string { header('Content-Type: text/html'); return ''; } }
function run(Value $obj): void {
header('Content-Type: application/json');
echo $obj; echo source();
}
