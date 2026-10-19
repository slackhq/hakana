use type Facebook\XHP\HTML\script;
<<Hakana\SecurityAnalysis\Sanitize('HtmlAttributeUri')>>
function checked_scheme(string $url): string { return $url; }
$url = (string)HH\global_get('_GET')['url'];
$script = <script src={checked_scheme($url)} />;
