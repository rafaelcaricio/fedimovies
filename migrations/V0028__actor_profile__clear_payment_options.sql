UPDATE actor_profile
SET payment_options = (
    -- remove all payment options except links
    SELECT COALESCE (
        jsonb_agg(opt) FILTER (WHERE opt ->> 'payment_type' = '1'),
        '[]'
    )
    FROM jsonb_array_elements(actor_profile.payment_options) AS opt
);
