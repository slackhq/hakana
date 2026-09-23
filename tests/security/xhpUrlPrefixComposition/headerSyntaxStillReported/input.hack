function header_sink(<<Hakana\SecurityAnalysis\Sink('ResponseHeader')>> string $header): void {}
function send(string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    header_sink('https://example.test/apps/'.$id.'?'.$query);
}
