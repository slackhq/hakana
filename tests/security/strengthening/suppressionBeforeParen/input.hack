function emit(<<Hakana\SecurityAnalysis\Sink('HtmlTag')>> string $s): void {}
$input = (string)HH\global_get('_GET')['q'];
emit /* HAKANA_SECURITY_IGNORE[HtmlTag] */ ($input); emit($input);
