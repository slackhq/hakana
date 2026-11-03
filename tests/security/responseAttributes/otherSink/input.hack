<<Hakana\SecurityAnalysis\ResponseHeader>>
function response_header(string $value, bool $replace = true): void { header($value, $replace); }
<<Hakana\SecurityAnalysis\ResponseOutput>>
function response_output(string $value): void { echo $value; }
function html_sink(<<Hakana\SecurityAnalysis\Sink('HtmlTag')>> string $value): void {}
function run(): void {
    $value = (string)HH\global_get('_POST')['value'];
    response_header('Content-Type: text/plain');
    response_output($value);
    html_sink($value);
}
