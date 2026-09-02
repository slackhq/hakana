function get_nullable_bool(): ?bool {
    return null;
}

function main(?bool $nullable, shape(?'value' => bool) $input, bool $other): void {
    if ($nullable) {
        echo "nullable";
    }

    if (!$nullable) {
        echo "negated";
    }

    if ($nullable && $other) {
        echo "combined";
    }

    if ($input['value'] ?? null) {
        echo "coalesce";
    }

    if (!($input['value'] ?? null)) {
        echo "negated coalesce";
    }

    if (get_nullable_bool()) {
        echo "call";
    }
}
