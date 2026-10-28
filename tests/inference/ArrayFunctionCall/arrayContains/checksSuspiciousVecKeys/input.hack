type UserIds = vec<int>;
newtype HiddenUserIds as vec<int> = vec<int>;

function checkId(vec<int> $ids, int $user_id): bool {
    return C\contains_key($ids, $user_id);
}

function checkField(vec<int> $ids, shape('user_id' => int) $row): bool {
    return C\contains_key($ids, $row['user_id']);
}

final class User {
    public function getId(): int {
        return 1234;
    }
}

function checkMethod(vec<int> $ids, User $user): bool {
    return C\contains_key($ids, $user->getId());
}

function checkLiteralContainer(int $user_id): bool {
    return C\contains_key(vec[1234, 5678], $user_id);
}

function checkEmptyVec(int $user_id): bool {
    return C\contains_key(vec[], $user_id);
}

function checkUnion(bool $choose_vec, vec<int> $ids, dict<int, bool> $map, int $user_id): bool {
    $items = $choose_vec ? $ids : $map;
    return C\contains_key($items, $user_id);
}

function checkAlias(UserIds $ids, int $user_id): bool {
    return C\contains_key($ids, $user_id);
}

function checkNewtype(HiddenUserIds $ids, int $user_id): bool {
    return C\contains_key($ids, $user_id);
}

function checkGeneric<T as vec<int>>(T $ids, int $user_id): bool {
    return C\contains_key($ids, $user_id);
}

function checkIdArithmetic(vec<int> $ids, int $user_id): bool {
    return C\contains_key($ids, $user_id + 1);
}

function checkMisleadingName(vec<int> $ids, int $index_id): bool {
    return C\contains_key($ids, $index_id);
}

function checkExplicitKeyType(vec<int> $ids, int $user_id): bool {
    return C\contains_key<int, arraykey, int>($ids, $user_id);
}

abstract class Collection {
    abstract const type TIds as vec<int>;

    public static function checkTypeConstant(this::TIds $ids, int $user_id): bool {
        return C\contains_key($ids, $user_id);
    }
}

function checkMutableId(vec<int> $ids, int $user_id): bool {
    $id = new HH\Lib\Ref($user_id);
    return C\contains_key($ids, $id->value);
}
