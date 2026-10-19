<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

class BasePrinter {
    public function write(string $value): void {}
}

final class PlainPrinter extends BasePrinter {}

trait LoggingPrinter {
    require extends BasePrinter;
    public function write(string $value): void { log_value($value); }
}

final class LoggedPrinter extends BasePrinter {
    use LoggingPrinter;
}

function write_unknown(BasePrinter $printer): void { $printer->write('public'); }

(new PlainPrinter())->write(private_value());
write_unknown(new LoggedPrinter());
