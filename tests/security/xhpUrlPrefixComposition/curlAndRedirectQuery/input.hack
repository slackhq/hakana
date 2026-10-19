function redirect(<<Hakana\SecurityAnalysis\Sink('RedirectUri')>> string $url): void {}
function send(string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    $url = 'https://example.test/apps/'.$id.'?'.$query;
    curl_init($url);
    redirect($url);
}
