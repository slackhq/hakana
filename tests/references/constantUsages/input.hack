class Config {
    const int MAX_ITEMS = 100;
    const string DEFAULT_NAME = "default";
}

function validate(int $count): bool {
    return $count <= Config::MAX_ITEMS;
}

function getName(?string $name): string {
    return $name ?? Config::DEFAULT_NAME;
}

function test(): void {
    $maxAllowed = Config::MAX_ITEMS;
    $defaultVal = Config::DEFAULT_NAME;
}
