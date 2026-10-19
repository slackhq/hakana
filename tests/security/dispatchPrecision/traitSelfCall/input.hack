<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

class BasePrinter {
    public static function write(string $value): void {}
}

trait SelfPrinter {
    require extends BasePrinter;
    public static function write_self(string $value): void { self::write($value); }
}

final class LoggedPrinter extends BasePrinter {
    use SelfPrinter;
    public static function write(string $value): void { log_value($value); }
}

LoggedPrinter::write_self(private_value());
