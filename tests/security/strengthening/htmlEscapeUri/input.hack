function url(<<Hakana\SecurityAnalysis\Sink('HtmlAttributeUri')>> string $url): void {}
url(htmlspecialchars((string)HH\global_get('_GET')['q'], ENT_QUOTES));
