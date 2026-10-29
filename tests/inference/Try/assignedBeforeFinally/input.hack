function takes_int(int $value): void {}
function cleanup(): void {}
function register((function(): void) $callback): void {}

function run(): void {
    $value = 'before';
    register(() ==> {
        try {
            $value = 1;
        } finally {
            cleanup();
        }
        takes_int($value);
    });
}

function fallback(?int $value): int {
    try {
    } finally {
        if ($value is null) {
            $value = 0;
        }
    }
    return $value;
}

function finally_replaces(): int {
    $value = 'before';
    try {
        $value = 'during';
    } finally {
        $value = 1;
    }
    return $value;
}
