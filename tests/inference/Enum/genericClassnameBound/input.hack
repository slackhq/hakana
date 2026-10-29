enum Choice: string {
    FIRST = 'first';
}

function enum_value<Tv as arraykey, Te as HH\BuiltinEnum<Tv>>(classname<Te> $enum): ?Tv {
    throw new Exception();
}

function choice(): ?Choice {
    return enum_value(Choice::class);
}
