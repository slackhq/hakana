<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

$values = keyset[private_value()];
log_value(HH\Lib\Str\join(HH\Lib\Keyset\keys($values), ','));
log_value(HH\Lib\Str\join(HH\Lib\Vec\keys($values), ','));
$keys = HH\Lib\Keyset\keys(dict[private_value() => 'public']);
log_value(HH\Lib\Str\join(HH\Lib\Keyset\keys($keys), ','));
