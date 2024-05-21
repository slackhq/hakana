final class Bar {
    public ?string $a = null;
}

function takesBar(?Bar $bar) : string {
    return $bar?->a ?? "default";
}