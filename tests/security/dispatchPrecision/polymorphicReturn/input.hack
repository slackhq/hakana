<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

class BaseValue {
    public function value(): string { return 'public'; }
}

final class PrivateValue extends BaseValue {
    public function value(): string { return private_value(); }
}

function log_unknown(BaseValue $value): void { log_value($value->value()); }

log_unknown(new PrivateValue());
