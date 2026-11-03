<<Hakana\SecurityAnalysis\ResponseHeader>>
function response_header(string $value, bool $replace = true): void { header($value, $replace); }
<<Hakana\SecurityAnalysis\ResponseOutput>>
function response_output(string $value): void { echo $value; }
function run(): void {
    $value = (string)HH\global_get('_POST')['value'];
    response_header('Content-Type: text/html');
    response_output($value);
}
