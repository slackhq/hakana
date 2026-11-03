<<Hakana\SecurityAnalysis\ResponseHeader>>
function response_header(string $value, bool $replace = true): void { header($value, $replace); }
<<Hakana\SecurityAnalysis\ResponseOutput>>
function response_output(string $value): void { echo $value; }
function run(bool $plain): void {
    $value = (string)HH\global_get('_POST')['value'];
    if ($plain) {
        response_header('Content-Type: text/plain');
    } else {
        response_header('Content-Type: application/json');
    }
    response_output($value);
}
