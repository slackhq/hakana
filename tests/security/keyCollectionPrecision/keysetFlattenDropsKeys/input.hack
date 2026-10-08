<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

$values = \HH\Lib\Keyset\flatten(vec[dict[private_value() => 'public']]);
log_value(\HH\Lib\Str\join(\HH\Lib\Keyset\keys($values), ','));
