<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

abstract class BaseValue {
    public function value(): string { return $this->wrapped(); }
    private function wrapped(): string { return $this->item(); }
    abstract protected function item(): string;
}

final class PlainValue extends BaseValue {
    protected function item(): string { return 'public'; }
}

final class PrivateValue extends BaseValue {
    public function wrapped(): string { return 'public'; }
    protected function item(): string { return private_value(); }
}

log_value((new PrivateValue())->value());
