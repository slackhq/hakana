function maybeThrows() : string {
    return "hello";
}

function b(bool $a): void {
    if (!$a) {
        return;
    }

    $b = "";

    try {
        $b = maybeThrows();
        echo $b;
    } catch (\Exception $e) {}

    echo $b;
}