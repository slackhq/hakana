function maybeThrows() : string {
    return "hello";
}

function b(): void {
    $b = "";

    try {
        $b = maybeThrows();
        echo $b;
    } catch (\Exception $e) {}

    echo $b;
}