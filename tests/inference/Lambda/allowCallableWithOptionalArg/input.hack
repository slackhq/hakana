function accept_closure((function():int) $x) : int {
    return $x();
}
function main(): void {
    accept_closure(
        (int $x = 5): int ==> {
            return $x;
        }
    );
}
