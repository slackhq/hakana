enum Original: string as string {
    FIRST = 'first';
}

enum Included: string as string {
    use Original;
    SECOND = 'second';
}

enum Nested: string as string {
    use Included;
}

function inherited_members(): vec<string> {
    return vec[Included::FIRST, Nested::FIRST, Nested::SECOND];
}

function missing_member(): void {
    echo Nested::MISSING;
}
