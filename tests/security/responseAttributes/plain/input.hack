<<Hakana\SecurityAnalysis\ResponseHeader>>
function response_header(string $value, bool $replace = true): void { header($value, $replace); }
<<Hakana\SecurityAnalysis\ResponseOutput>>
function response_output(string $value): void { echo $value; }
function run(): void {
    $value = (string)HH\global_get('_POST')['value'];
    response_header('Content-Type: TeXt/PlAiN; charset=utf-8');
    response_output($value);
    response_output($value);
    echo $value;
}
