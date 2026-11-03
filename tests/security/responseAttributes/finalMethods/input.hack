final class Response {
    <<Hakana\SecurityAnalysis\ResponseHeader>>
    public function setHeader(string $value, bool $replace = true): void { header($value, $replace); }
    <<Hakana\SecurityAnalysis\ResponseOutput>>
    public function output(string $value): void { echo $value; }
}
function run(): void {
    $response = new Response();
    $value = (string)HH\global_get('_POST')['value'];
    $response->setHeader('Content-Type: text/plain');
    $response->output($value);
}
