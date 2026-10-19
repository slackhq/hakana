<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

$values = dict['public_key' => private_value()];
log_value(HH\Lib\Str\join(HH\Lib\Keyset\keys($values), ','));
log_value(HH\Lib\Str\join(HH\Lib\Vec\keys($values), ','));
