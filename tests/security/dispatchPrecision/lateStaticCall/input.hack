<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

class BasePrinter {
    public static function write(string $value): void {}
    public static function write_static(string $value): void { static::write($value); }
}

final class LoggedPrinter extends BasePrinter {
    public static function write(string $value): void { log_value($value); }
}

LoggedPrinter::write_static(private_value());
