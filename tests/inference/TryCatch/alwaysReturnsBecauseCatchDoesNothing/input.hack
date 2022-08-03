function throws(): void {
    throw new Exception("bad");
}
function foo(): string {
    try {
        throws();
    } catch (Exception $e) {
        // do nothing
    }

    return "hello";
}