<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

class BaseValue {
    public function value(): string { return 'public'; }
}

final class PlainValue extends BaseValue {}

final class PrivateValue extends BaseValue {
    public function value(): string { return private_value(); }
}

function get_unknown(BaseValue $value): string { return $value->value(); }

get_unknown(new PrivateValue());
log_value((new PlainValue())->value());
