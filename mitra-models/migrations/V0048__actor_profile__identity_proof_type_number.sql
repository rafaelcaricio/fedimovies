UPDATE actor_profile
SET identity_proofs = replaced.identity_proofs
FROM (
    SELECT
        actor_profile.id,
        jsonb_agg(
            CASE
            WHEN identity_proof ->> 'proof_type' = 'ethereum-eip191-00'
            THEN jsonb_set(identity_proof, '{proof_type}', '1')
            WHEN identity_proof ->> 'proof_type' = 'MitraMinisignSignature2022A'
            THEN jsonb_set(identity_proof, '{proof_type}', '2')
            END
        ) AS identity_proofs
    FROM actor_profile
    CROSS JOIN jsonb_array_elements(actor_profile.identity_proofs) AS identity_proof
    GROUP BY actor_profile.id
) AS replaced
WHERE actor_profile.id = replaced.id;
