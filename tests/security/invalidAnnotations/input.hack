<<Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
function input(): string { return ''; }
<<Hakana\SecurityAnalysis\Sanitize('CurlURL')>>
function bad_sanitizer(string $value): string { return $value; }
function empty_validator(
    <<Hakana\SecurityAnalysis\RemoveTaintsWhenReturningTrue()>>
    string $value,
): bool { return true; }
<<Hakana\SecurityAnalysis\Sanitise('HtmlTag')>>
function typo(string $value): string { return $value; }
<<Hakana\SecurityAnalysis\NotSourceWhen('missing', 'trusted')>>
function bad_parameter(string $key): string { return $key; }
echo bad_sanitizer(input());
