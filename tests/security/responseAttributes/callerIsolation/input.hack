<<Hakana\SecurityAnalysis\ResponseHeader>>
function response_header(string $value, bool $replace = true): void { header($value, $replace); }
<<Hakana\SecurityAnalysis\ResponseOutput>>
function response_output(string $value): void { echo $value; }
function safe(): void {
    $value = (string)HH\global_get('_POST')['value'];
    response_header('Content-Type: text/plain');
    response_output($value);
}
function unsafe(): void {
    $value = (string)HH\global_get('_POST')['value'];
    response_output($value);
}
