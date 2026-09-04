function label(E $e): string {
    switch ($e) {
        case E::A:
            return "a";
        case E::B:
            return "b";
    }
}

<<__EntryPoint>>
function main(): void {
    echo label(E::A);
}
