trait HasPriority {
    protected static int $priority = 1;

    final public function getPriority(): int {
        return static::$priority;
    }
}

abstract class BaseEmail {
    use HasPriority;
}

final class UrgentEmail extends BaseEmail {
    protected static int $priority = 3;
    protected static int $never_read = 3;
}

abstract class BaseLinter {
    public static bool $is_progressive = true;
    public static bool $never_read = true;
}

final class FooLinter extends BaseLinter {
    public static bool $is_progressive = false;
}

function is_progressive(classname<BaseLinter> $linter): bool {
    $linter = HH\classname_to_class($linter);
    return $linter::$is_progressive;
}

final class SetupRequest {
    public string $body = '';

    public function __construct(protected string $kind, protected string $never_read) {
        $this->body = $this->kind;
    }
}

<<__EntryPoint>>
function main(): void {
    echo (new UrgentEmail())->getPriority();
    echo is_progressive(FooLinter::class) ? 'yes' : 'no';
    echo (new SetupRequest('a', 'b'))->body;
    echo Rules::evaluate();
}

abstract class Conjunction {
    protected static vec<string> $propositions = vec[];
    protected static vec<string> $never_read = vec[];
}

abstract class AndConjunction extends Conjunction {
    public static function evaluate(): int {
        return HH\Lib\C\count(static::$propositions);
    }
}

final class Rules extends AndConjunction {
    protected static vec<string> $propositions = vec['a', 'b'];
}
