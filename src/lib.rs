// -*- coding: utf-8; mode: rust; -*-
//
// To the extent possible under law, the authors have waived all
// copyright and related or neighboring rights to zkp,
// using the Creative Commons "CC0" public domain dedication.  See
// <http://creativecommons.org/publicdomain/zero/.0/> for full
// details.
//
// Authors:
// - Henry de Valence <hdevalence@hdevalence.ca>

//! This crate has an experimental zero-knowledge proof compiler
//! implemented using Rust macros.
//!
//! It provides a DSL resembing Camenisch-Stadler notation for proving
//! statements about discrete logarithms in the Decaf group on
//! Curve25519, as implemented in
//! [`curve25519-dalek`](https://github.com/isislovecruft/curve25519-dalek).
//! Note that both the Decaf implementation in `curve25519-dalek`, *as
//! well as this library*, are currently **UNFINISHED, UNREVIEWED, AND
//! EXPERIMENTAL**.  (I haven't actually checked carefully that the
//! proofs are sound, for instance...)
#![allow(non_snake_case)]
#![feature(test)]

extern crate serde;

#[doc(hidden)]
#[macro_use]
pub extern crate serde_derive;
#[doc(hidden)]
pub extern crate curve25519_dalek;
#[doc(hidden)]
pub extern crate rand;
#[doc(hidden)]
pub extern crate sha2;

/// compute_formula_consttime!((publics, scalars) (A*a + B*b + ...))
/// returns
/// The input to this macro is of the form
///
///   (publics, scalars) (A*a + B*b + ...)
///
/// where `publics` is the name of a struct with members `A, B, ...`
/// of type `&DecafPoint`, and `scalars` is the name of a struct with
/// members `a, b, ...` of type `Scalar`.
///
/// It expands to an expression of the form
///
///   &(publics.A * &scalars.a) + &( &(publics.B * &scalars.b) + &(...))
///
/// All these operations are constant-time.
#[doc(hidden)]
#[macro_export]
macro_rules! __compute_formula_consttime {
    // Unbracket a statement
    (($publics:ident, $scalars:ident) ($($x:tt)*)) => {
        __compute_formula_consttime!(($publics,$scalars) $($x)*)
    };
    // Multi-part statement
    (($publics:ident, $scalars:ident)
     $point:ident * $scalar:ident + $($x:tt)*) => {
        &($publics.$point * &$scalars.$scalar) +
        &( __compute_formula_consttime!(($publics,$scalars) $($x)*) )
    };
    // Single-part statement / end of statement
    (($publics:ident, $scalars:ident)
     $point:ident * $scalar:ident ) => {
        $publics.$point * &$scalars.$scalar
    };
}

/// Expands to a constructor for a `Commitments` struct, which
/// computes (in constant time) commitments based on the input
/// statements.
#[doc(hidden)]
#[macro_export]
macro_rules! __compute_commitments_consttime {
    (($publics:ident, $scalars:ident) $($lhs:ident = $statement:tt),+) => {
        Commitments {
            $( $lhs : __compute_formula_consttime!(($publics, $scalars) $statement) ),+
        }
    }
}

/// Creates a module with code required to produce a non-interactive
/// zero-knowledge proof statement, to serialize it to wire format, to
/// parse from wire format, and to verify the proof statement.
///
/// The statement is specified in an embedded DSL resembling
/// Camenisch-Stadler notation.  For instance, a proof of knowledge of
/// two equal discrete logarithms ("DLEQ") is specified as:
///
/// ```rust,ignore
/// create_nipk!{dleq, (x), (A, B, G, H) : A = (G * x), B = (H * x) }
/// ```
///
/// This creates a module `dleq` with code for proving knowledge of a
/// secret `x: Scalar` such that `A = G*x`, `B = H*x` for public
/// parameters `A, B, G, H: DecafPoint`.  In general the syntax is
///
/// ```rust,ignore
/// create_nipk!{
///     module_name, // used to label proof statements
///     (x,y,z,...), // secret variable names
///     (A,B,C,...)  // public parameter names
///     :
///     LHS = (A * x + B * y + C * z + ... ),  // comma-seperated statements
///     ...
/// }
/// ```
///
/// Statements have the form `LHS = (A * x + B * y + C * z + ... )`,
/// where `LHS` is one of the points listed as a public parameter, and
/// the right-hand side is a sum of public points multiplied by secret
/// scalars.
///
/// Inside the generated module `module_name`, the macro defines three
/// structs:
///
/// A `Publics` struct corresponding to the public parameters, of the
/// form
///
/// ```rust,ignore
/// pub struct Publics<'a> { pub A: &'a DecafPoint, ... }
/// ```
///
/// A `Secrets` struct corresponding to the secret parameters, of the
/// form
///
/// ```rust,ignore
/// pub struct Secrets<'a> { pub x: &'a Scalar, ... }
/// ```
///
/// A `Proof` struct, of the form
///
/// ```rust,ignore
/// #[derive(Serialize, Deserialize)]
/// pub struct Proof { ... }
///
/// impl Proof {
///     pub fn create<R: Rng>(
///         csprng: &mut R,
///         publics: Publics,
///         secrets: Secrets,
///     ) -> Proof { ... }
///
///     pub fn verify(&self, publics: Publics) -> Result<(),()> { ... }
/// }
/// ```
///
/// The `Proof` struct derives the Serde traits, so it can be
/// serialized and deserialized to various wire formats.
///
/// The `Publics` and `Secrets` structs are used to fake named
/// arguments in the input to `create` and `verify`.  Proof creation
/// is done in constant time.
///
/// As an example, we can create and verify a DLEQ proof as follows:
///
/// ```
/// #[macro_use]
/// extern crate serde_derive;
///
/// #[macro_use]
/// extern crate zkp;
///
/// extern crate curve25519_dalek;
/// use curve25519_dalek::constants as dalek_constants;
/// use curve25519_dalek::decaf::DecafPoint;
/// use curve25519_dalek::scalar::Scalar;
///
/// extern crate rand;
/// use rand::OsRng;
///
/// extern crate sha2;
/// use sha2::Sha256;
///
/// extern crate serde_cbor;
///
/// # fn main() {
/// let mut csprng = OsRng::new().unwrap();
/// let G = &dalek_constants::DECAF_ED25519_BASEPOINT;
/// let H = DecafPoint::hash_from_bytes::<Sha256>(G.compress().as_bytes());
///
/// create_nipk!{dleq, (x), (A, B, G, H) : A = (G * x), B = (H * x) }
///
/// let x = Scalar::from_u64(89327492234);
/// let A =  G * &x;
/// let B = &H * &x;
///
/// let publics = dleq::Publics{A: &A, B: &B, G: G, H: &H};
/// let secrets = dleq::Secrets{x: &x};
///
/// let proof = dleq::Proof::create(&mut csprng, publics, secrets);
///
/// // Serialize to packed CBOR byte representation
/// let proof_bytes = serde_cbor::ser::to_vec_packed(&proof).unwrap();
///
/// // Send bytes over the wire here ...
///
/// // Parse bytes back to in-memory representation
/// let parsed_proof: dleq::Proof
///     = serde_cbor::from_slice(&proof_bytes).unwrap();
///
/// // Check the proof.
/// assert!(parsed_proof.verify(publics).is_ok());
/// # }
/// ```
#[macro_export]
macro_rules! create_nipk {
    (
        $proof_module_name:ident // Name of the module to create
        ,
        ( $($secret:ident),+ ) // Secret variables, sep by commas
        ,
        ( $($public:ident),+ ) // Public variables, sep by commas
        :
        // List of statements to prove
        // Format: LHS = ( ... RHS expr ... ), 
        $($lhs:ident = $statement:tt),+
    ) => {
        mod $proof_module_name {
            use $crate::curve25519_dalek::scalar::Scalar;
            use $crate::curve25519_dalek::decaf::DecafPoint;
            use $crate::sha2::{Digest, Sha512};
            use $crate::rand::Rng;

            #[derive(Copy, Clone)]
            pub struct Secrets<'a> {
                // Create a parameter for each secret value
                $(
                    pub $secret : &'a Scalar,
                )+
            }

            #[derive(Copy, Clone)]
            pub struct Publics<'a> {
                // Create a parameter for each public value
                $(
                    pub $public : &'a DecafPoint,
                )+
            }

            // Hack because we can't concat identifiers,
            // so do responses.x instead of responses_x
            // rand.x instead of rand_x, etc.

            struct Commitments {$($lhs: DecafPoint,)+ }
            struct Randomnesses {$($secret : Scalar,)+}
            #[derive(Serialize, Deserialize)]
            struct Responses {$($secret : Scalar,)+}

            #[derive(Serialize, Deserialize)]
            pub struct Proof {
                challenge: Scalar,
                responses: Responses,
            }

            impl Proof {
                #[allow(dead_code)]
                pub fn create<R: Rng>(
                    csprng: &mut R,
                    publics: Publics,
                    secrets: Secrets,
                ) -> Proof {
                    let rand = Randomnesses{
                        $(
                            $secret : Scalar::random(csprng),
                        )+
                    };
                    // $statement_rhs = `X * x + Y * y + Z * z`
                    // should become
                    // `publics.X * rand.x + publics.Y * rand.y + publics.Z * rand.z`
                    let commitments = __compute_commitments_consttime!(
                        (publics, rand) $($lhs = $statement),*
                    );

                    let mut hash = Sha512::default();

                    $(
                        hash.input(publics.$public.compress().as_bytes());
                    )+
                    $(
                        hash.input(commitments.$lhs.compress().as_bytes());
                    )+

                    let challenge = Scalar::from_hash(hash);

                    let responses = Responses{
                        $(
                            $secret : Scalar::multiply_add(
                                &challenge,
                                &secrets.$secret,
                                &rand.$secret
                            ),
                        )+
                    };

                    Proof{ challenge: challenge, responses: responses }
                }

                #[allow(dead_code)]
                pub fn verify(&self, publics: Publics) -> Result<(),()> {
                    // `A = X * x + Y * y`
                    // should become
                    // `publics.X * responses.x + publics.Y * responses.y - publics.A * self.challenge`
                    let responses = &self.responses;
                    let mut commitments = __compute_commitments_consttime!(
                        (publics, responses) $($lhs = $statement),*
                    );
                    $(
                        commitments.$lhs -= &(publics.$lhs * &self.challenge);
                    )*
                    
                    let mut hash = Sha512::default();
                    // Add each public point into the hash
                    $(
                        hash.input(publics.$public.compress().as_bytes());
                    )+
                    // Add each (recomputed) commitment into the hash
                    $(
                        hash.input(commitments.$lhs.compress().as_bytes());
                    )*
                        
                    // Recompute challenge
                    let challenge = Scalar::from_hash(hash);

                    if challenge == self.challenge { Ok(()) } else { Err(()) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate serde_cbor;
    extern crate test;

    use rand::OsRng;
    use sha2::Sha256;
    use self::test::Bencher;

    use curve25519_dalek::constants as dalek_constants;
    use curve25519_dalek::decaf::DecafPoint;
    use curve25519_dalek::scalar::Scalar;
    
    #[bench]
    fn create_gen_dleq(b: &mut Bencher) {
        let mut csprng = OsRng::new().unwrap();
        let G = &dalek_constants::DECAF_ED25519_BASEPOINT;
        let H = DecafPoint::hash_from_bytes::<Sha256>(G.compress().as_bytes());

        create_nipk!{dleq, (x), (A, B, G, H) : A = (G * x), B = (H * x) }

        let x = Scalar::from_u64(89327492234);
        let A =  G * &x;
        let B = &H * &x;

        let publics = dleq::Publics{A: &A, B: &B, G: G, H: &H};
        let secrets = dleq::Secrets{x: &x};

        b.iter(|| dleq::Proof::create(&mut csprng, publics, secrets));
    }
    
    #[bench]
    fn verify_gen_dleq(b: &mut Bencher) {
        let mut csprng = OsRng::new().unwrap();
        let G = &dalek_constants::DECAF_ED25519_BASEPOINT;
        let H = DecafPoint::hash_from_bytes::<Sha256>(G.compress().as_bytes());

        create_nipk!{dleq, (x), (A, B, G, H) : A = (G * x), B = (H * x) }

        let x = Scalar::from_u64(89327492234);
        let A =  G * &x;
        let B = &H * &x;

        let publics = dleq::Publics{A: &A, B: &B, G: G, H: &H};
        let secrets = dleq::Secrets{x: &x};

        let proof = dleq::Proof::create(&mut csprng, publics, secrets);
        b.iter(|| proof.verify(publics).is_ok());
    }

    #[test]
    fn create_and_verify_gen_dleq() {
        let mut csprng = OsRng::new().unwrap();
        let G = &dalek_constants::DECAF_ED25519_BASEPOINT;
        let H = DecafPoint::hash_from_bytes::<Sha256>(G.compress().as_bytes());

        create_nipk!{dleq, (x), (A, B, G, H) : A = (G * x), B = (H * x) }

        let x = Scalar::from_u64(89327492234);
        let A =  G * &x;
        let B = &H * &x;

        let publics = dleq::Publics{A: &A, B: &B, G: G, H: &H};
        let secrets = dleq::Secrets{x: &x};

        let proof = dleq::Proof::create(&mut csprng, publics, secrets);
        // serialize to packed CBOR byte representation
        let proof_bytes = serde_cbor::ser::to_vec_packed(&proof).unwrap();
        // parse bytes back to memory
        let parsed_proof: dleq::Proof
            = serde_cbor::from_slice(&proof_bytes).unwrap();

        assert!(parsed_proof.verify(publics).is_ok());
    }
}
