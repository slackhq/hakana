interface ConfigSection {}
final class WhoIsCoolConfig extends ConfigSection {
    public function __construct(private string $name = "Matt") {}

    public function getName(): string {
        return $this->name;
    }
}

final class Config {
    private static dict<string, ConfigSection> $config_sections = dict[];

    private static function initConfig(): void {
        self::$config_sections[WhoIsCoolConfig::class] = new WhoIsCoolConfig();
    }

    private static function coerce<<<__Enforceable>> reify T as ConfigSection>(ConfigSection $config_section): T {
        $config_section = $config_section ?as T;

        if ($config_section is null) {
            throw new \Exception('bad');
        }

        return $config_section;
    }

    public static function get<<<__Enforceable>> reify T as ConfigSection>(): T {
        self::initConfig();
        $classname = HH\ReifiedGenerics\get_classname<T>();
        $config_section = self::$config_sections[$classname];
        return self::coerce<T>($config_section);
    }
}

function who_is_cool(): string {
    $config = Config::get<WhoIsCoolConfig>();
    return $config->getName();
}