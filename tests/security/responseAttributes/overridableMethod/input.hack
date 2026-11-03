class Response {
    <<Hakana\SecurityAnalysis\ResponseHeader>>
    public function setHeader(string $value): void { header($value); }
}
class HtmlResponse extends Response {
    public function setHeader(string $value): void { header('Content-Type: text/html'); }
}
function run(Response $response): void {
    $value = (string)HH\global_get('_POST')['value'];
    $response->setHeader('Content-Type: text/plain');
    echo $value;
}
