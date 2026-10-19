<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

class BasePrinter {
    public function write(string $value): void {}
}

trait LoggingPrinter {
    require extends BasePrinter;
    public function write(string $value): void { log_value($value); }
}

final class LoggedPrinter extends BasePrinter {
    use LoggingPrinter;
}

function write_unknown(BasePrinter $printer, string $value): void { $printer->write($value); }

write_unknown(new LoggedPrinter(), private_value());
