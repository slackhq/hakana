<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

class BasePrinter {
    public function write(string $value): void {}
}

final class LoggedPrinter extends BasePrinter {
    public function write(string $value): void { log_value($value); }
    public function write_parent(string $value): void { parent::write($value); }
}

(new LoggedPrinter())->write_parent(private_value());
