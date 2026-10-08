<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_values(): dict<string, string> { return dict[]; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

log_value(\HH\Lib\Str\join(\HH\Lib\Keyset\keys(private_values()), ','));
log_value(\HH\Lib\Str\join(\HH\Lib\Vec\keys(private_values()), ','));
