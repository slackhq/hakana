use type Facebook\XHP\HTML\script;
<<Hakana\SecurityAnalysis\Sanitize('HtmlActiveResourceUri')>>
function reviewed_resource(string $url): string { return $url; }
$url = (string)HH\global_get('_GET')['url'];
$script = <script src={reviewed_resource($url)} />;
