<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
function logger(<<Hakana\SecurityAnalysis\Sink('Logging', 'HtmlTag')>> string $s): void {}
logger((string)mb_substr(input(), 0, 2));
