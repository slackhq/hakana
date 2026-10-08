<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

$values = dict['outer' => dict[private_value() => 'public_value']];
$keys = vec[HH\Lib\Keyset\keys($values['outer'])];
log_value(HH\Lib\Str\join($keys[0], ','));
