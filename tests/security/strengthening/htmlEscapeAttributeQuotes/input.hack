function attr(<<Hakana\SecurityAnalysis\Sink('HtmlAttribute')>> string $value): void {}
attr(htmlspecialchars((string)HH\global_get('_GET')['value'], 3));
