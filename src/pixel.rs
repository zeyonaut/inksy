// Copyright (C) 2023 Aaron Yeoh Cruz <zeyonaut@gmail.com>
// SPDX-License-Identifier: MPL-2.0

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use core::ops::{Add, Div, Mul, Sub};
use std::ops::Neg;

// Types implementing Zero have an additive unit.
pub trait Zero {
	const ZERO: Self;
}

impl Zero for f32 {
	const ZERO: Self = 0.;
}

pub trait Area {
	type Length;
	fn sqrt(self) -> Self::Length;
}

macro_rules! impl_new_f32 {
	($Name:ident) => {
		impl $Name {
			pub fn min(self, other: Self) -> Self {
				Self(self.0.min(other.0))
			}

			pub fn max(self, other: Self) -> Self {
				Self(self.0.max(other.0))
			}

			pub fn abs(self) -> Self {
				Self(self.0.abs())
			}
		}

		impl Zero for $Name {
			const ZERO: Self = Self(0.);
		}

		impl<'a> Add<&'a $Name> for $Name {
			type Output = $Name;

			fn add(self, rhs: &'a $Name) -> Self::Output {
				$Name(self.0 + rhs.0)
			}
		}

		impl<'a> Add<$Name> for &'a $Name {
			type Output = $Name;

			fn add(self, rhs: $Name) -> Self::Output {
				$Name(self.0 + rhs.0)
			}
		}
		impl<'a> Sub<&'a $Name> for $Name {
			type Output = $Name;

			fn sub(self, rhs: &'a $Name) -> Self::Output {
				$Name(self.0 - rhs.0)
			}
		}

		impl<'a> Sub<$Name> for &'a $Name {
			type Output = $Name;

			fn sub(self, rhs: $Name) -> Self::Output {
				$Name(self.0 - rhs.0)
			}
		}

		impl Div<f32> for $Name {
			type Output = Self;

			fn div(self, rhs: f32) -> Self::Output {
				Self(self.0 / rhs)
			}
		}

		impl Div<$Name> for $Name {
			type Output = f32;

			fn div(self, rhs: $Name) -> Self::Output {
				self.0 / rhs.0
			}
		}

		impl<'a> Div<&'a $Name> for $Name {
			type Output = f32;

			fn div(self, rhs: &'a $Name) -> Self::Output {
				self.0 / rhs.0
			}
		}

		impl<'a> Div<&'a mut $Name> for $Name {
			type Output = f32;

			fn div(self, rhs: &'a mut $Name) -> Self::Output {
				self.0 / rhs.0
			}
		}

		impl Mul<f32> for $Name {
			type Output = Self;

			fn mul(self, rhs: f32) -> Self::Output {
				Self(self.0 * rhs)
			}
		}

		impl<'a> Mul<f32> for &'a $Name {
			type Output = $Name;

			fn mul(self, rhs: f32) -> Self::Output {
				$Name(self.0 * rhs)
			}
		}

		impl Mul<$Name> for f32 {
			type Output = $Name;

			fn mul(self, rhs: $Name) -> Self::Output {
				$Name(self * rhs.0)
			}
		}
	};
}

macro_rules! impl_new_f32_dimensionality {
	{length: $Length:ident, area: $Area:ident} => {
		impl Mul<$Length> for $Length {
			type Output = $Area;

			fn mul(self, rhs: $Length) -> Self::Output {
				$Area(self.0 * rhs.0)
			}
		}

		impl Area for $Area {
			type Length = $Length;
			fn sqrt(self) -> Self::Length {
				$Length(self.0.sqrt())
			}
		}
	}
}

// A virtual pixel length.
#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Add, derive_more::Sub, derive_more::Neg, derive_more::From, derive_more::Into, PartialEq, PartialOrd, Debug, derive_more::Display, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vx(pub f32);

// A virtual pixel area.
#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Add, derive_more::Sub, derive_more::Neg, derive_more::From, derive_more::Into, PartialEq, PartialOrd, Debug, derive_more::Display, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vx2(pub f32);

impl_new_f32!(Vx);
impl_new_f32!(Vx2);
impl_new_f32_dimensionality! {length: Vx, area: Vx2}

// A logical pixel length.
#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Add, derive_more::Sub, derive_more::Neg, derive_more::From, derive_more::Into, PartialEq, PartialOrd, Debug, derive_more::Display, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Lx(pub f32);

// A logical pixel area.
#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Add, derive_more::Sub, derive_more::Neg, derive_more::From, derive_more::Into, PartialEq, PartialOrd, Debug, derive_more::Display, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Lx2(pub f32);

impl_new_f32!(Lx);
impl_new_f32!(Lx2);
impl_new_f32_dimensionality! {length: Lx, area: Lx2}

// A zoom factor is a ratio between a logical pixel and a virtual pixel.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Zoom(pub f32);

impl Vx {
	pub fn z(self, zoom: Zoom) -> Lx {
		Lx(self.0 * zoom.0)
	}
}

impl Lx {
	pub fn z(self, zoom: Zoom) -> Vx {
		Vx(self.0 / zoom.0)
	}
}

// A scale is a ratio between a physical pixel and a logical pixel.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Scale(pub f32);

impl Lx {
	pub fn s(self, scale: Scale) -> Px {
		Px(self.0 * scale.0)
	}
}

impl Px {
	pub fn s(self, scale: Scale) -> Lx {
		Lx(self.0 / scale.0)
	}
}

// A physical pixel length.
#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Add, derive_more::Sub, derive_more::Neg, derive_more::From, derive_more::Into, PartialEq, PartialOrd, Debug, derive_more::Display, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Px(pub f32);

// A physical pixel area.
#[repr(transparent)]
#[derive(Clone, Copy, derive_more::Add, derive_more::Sub, derive_more::Neg, derive_more::From, derive_more::Into, PartialEq, PartialOrd, Debug, derive_more::Display, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Px2(pub f32);

impl_new_f32!(Px);
impl_new_f32!(Px2);
impl_new_f32_dimensionality! {length: Px, area: Px2}

fn map_r<'a, const N: usize, A, U, F: FnMut(&'a A) -> U>(a: &'a [A; N], mut f: F) -> [U; N] {
	use core::mem::MaybeUninit;
	let mut x = MaybeUninit::<[U; N]>::uninit().transpose();
	for i in 0..N {
		// SAFETY: l and a are of length N where i < N.
		unsafe {
			x.get_unchecked_mut(i).write(f(a.get_unchecked(i)));
		}
	}
	// SAFETY: x is fully initialized by the preceding loop.
	unsafe { x.transpose().assume_init() }
}

fn map2<const N: usize, A, B, U, F: FnMut(A, B) -> U>(l: [A; N], r: [B; N], mut f: F) -> [U; N] {
	use core::mem::MaybeUninit;
	let mut l = MaybeUninit::new(l).transpose();
	let mut r = MaybeUninit::new(r).transpose();
	let mut x = MaybeUninit::<[U; N]>::uninit().transpose();
	for i in 0..N {
		// SAFETY: l, r, and x are of length N where i < N; each element of l and r is initialized when read and only read once.
		unsafe {
			x.get_unchecked_mut(i).write(f(l.get_unchecked_mut(i).assume_init_read(), r.get_unchecked_mut(i).assume_init_read()));
		}
	}
	// SAFETY: x is fully initialized by the preceding loop.
	unsafe { x.transpose().assume_init() }
}

fn map2_r0<'a, const N: usize, A, B, U, F: FnMut(&'a A, B) -> U>(l: &'a [A; N], r: [B; N], mut f: F) -> [U; N] {
	use core::mem::MaybeUninit;
	let mut r = MaybeUninit::new(r).transpose();
	let mut x = MaybeUninit::<[U; N]>::uninit().transpose();
	for i in 0..N {
		// SAFETY: l, r, and x are of length N where i < N; each element of r is initialized when read and only read once.
		unsafe {
			x.get_unchecked_mut(i).write(f(l.get_unchecked(i), r.get_unchecked_mut(i).assume_init_read()));
		}
	}
	// SAFETY: x is fully initialized by the preceding loop.
	unsafe { x.transpose().assume_init() }
}

fn map2_r1<'b, const N: usize, A, B, U, F: FnMut(A, &'b B) -> U>(l: [A; N], r: &'b [B; N], mut f: F) -> [U; N] {
	use core::mem::MaybeUninit;
	let mut l = MaybeUninit::new(l).transpose();
	let mut x = MaybeUninit::<[U; N]>::uninit().transpose();
	for i in 0..N {
		// SAFETY: l, r, and x are of length N where i < N; each element of l is initialized when read and only read once.
		unsafe {
			x.get_unchecked_mut(i).write(f(l.get_unchecked_mut(i).assume_init_read(), r.get_unchecked(i)));
		}
	}
	// SAFETY: x is fully initialized by the preceding loop.
	unsafe { x.transpose().assume_init() }
}

fn map2_r0_r1<'a, 'b, const N: usize, A, B, U, F: FnMut(&'a A, &'b B) -> U>(l: &'a [A; N], r: &'b [B; N], mut f: F) -> [U; N] {
	use core::mem::MaybeUninit;
	let mut x = MaybeUninit::<[U; N]>::uninit().transpose();
	for i in 0..N {
		// SAFETY: l, r, and x are of length N where i < N.
		unsafe {
			x.get_unchecked_mut(i).write(f(l.get_unchecked(i), r.get_unchecked(i)));
		}
	}
	// SAFETY: x is fully initialized by the preceding loop.
	unsafe { x.transpose().assume_init() }
}

#[repr(transparent)]
#[derive(Clone, Copy, derive_more::From, derive_more::Into, derive_more::Index, PartialEq, PartialOrd, Debug)]
pub struct Vex<const N: usize, T>(pub [T; N]);

impl<const N: usize, T> Vex<N, T> {
	pub fn map<A, F: FnMut(T) -> A>(self, f: F) -> Vex<N, A> {
		Vex(self.0.map(f))
	}

	pub fn dot<A, S: Zero + Add<Output = S>>(self, other: Vex<N, A>) -> S
	where
		T: Zero + Mul<A, Output = S>,
	{
		map2(self.0, other.0, |a, b| a * b).into_iter().fold(S::ZERO, |acc, x| acc + x)
	}

	pub fn norm<S: Zero + Add<Output = S> + Area<Length = T>>(self) -> T
	where
		T: Zero + Mul<T, Output = S> + Clone,
	{
		self.0.into_iter().fold(S::ZERO, |acc, x| acc + x.clone() * x).sqrt()
	}

	pub fn normalized<K, S: Zero + Add<Output = S> + Area<Length = T>>(self) -> Vex<N, K>
	where
		T: Zero + Mul<T, Output = S> + Div<T, Output = K> + Clone,
	{
		self.clone() / self.norm()
	}
}

impl<A> Vex<2, A> {
	pub fn cross<B, P, D>(self, other: Vex<2, B>) -> D
	where
		A: Mul<B, Output = P>,
		P: Sub<P, Output = D>,
	{
		let (Vex([l_x, l_y]), Vex([r_x, r_y])) = (self, other);
		l_x * r_y - l_y * r_x
	}
}

// Add:

impl<const N: usize, T> Add<Vex<N, T>> for Vex<N, T>
where
	T: Add<Output = T>,
{
	type Output = Vex<N, T>;

	fn add(self, rhs: Vex<N, T>) -> Self::Output {
		Vex(map2(self.0, rhs.0, |a, b| a + b))
	}
}

impl<'a, const N: usize, T> Add<&'a Vex<N, T>> for Vex<N, T>
where
	T: Add<&'a T, Output = T>,
{
	type Output = Vex<N, T>;

	fn add(self, rhs: &'a Vex<N, T>) -> Self::Output {
		Vex(map2_r1(self.0, &rhs.0, |a, b| a + b))
	}
}

impl<'a, const N: usize, T> Add<Vex<N, T>> for &'a Vex<N, T>
where
	&'a T: Add<T, Output = T>,
{
	type Output = Vex<N, T>;

	fn add(self, rhs: Vex<N, T>) -> Self::Output {
		Vex(map2_r0(&self.0, rhs.0, |a, b| a + b))
	}
}

impl<'a, 'b, const N: usize, T: Add<Output = T>> Add<&'a Vex<N, T>> for &'b Vex<N, T>
where
	&'b T: Add<&'a T, Output = T>,
{
	type Output = Vex<N, T>;

	fn add(self, rhs: &'a Vex<N, T>) -> Self::Output {
		Vex(map2_r0_r1(&self.0, &rhs.0, |a, b| a + b))
	}
}

// Sub:

impl<const N: usize, T> Sub<Vex<N, T>> for Vex<N, T>
where
	T: Sub<Output = T>,
{
	type Output = Vex<N, T>;

	fn sub(self, rhs: Vex<N, T>) -> Self::Output {
		Vex(map2(self.0, rhs.0, |a, b| a - b))
	}
}

impl<'a, const N: usize, T> Sub<&'a Vex<N, T>> for Vex<N, T>
where
	T: Sub<&'a T, Output = T>,
{
	type Output = Vex<N, T>;

	fn sub(self, rhs: &'a Vex<N, T>) -> Self::Output {
		Vex(map2_r1(self.0, &rhs.0, |a, b| a - b))
	}
}

impl<'a, const N: usize, T> Sub<Vex<N, T>> for &'a Vex<N, T>
where
	&'a T: Sub<T, Output = T>,
{
	type Output = Vex<N, T>;

	fn sub(self, rhs: Vex<N, T>) -> Self::Output {
		Vex(map2_r0(&self.0, rhs.0, |a, b| a - b))
	}
}

impl<'a, 'b, const N: usize, T: Sub<Output = T>> Sub<&'a Vex<N, T>> for &'b Vex<N, T>
where
	&'b T: Sub<&'a T, Output = T>,
{
	type Output = Vex<N, T>;

	fn sub(self, rhs: &'a Vex<N, T>) -> Self::Output {
		Vex(map2_r0_r1(&self.0, &rhs.0, |a, b| a - b))
	}
}

// Neg:

impl<const N: usize, S, T: Neg<Output = S>> Neg for Vex<N, T> {
	type Output = Vex<N, S>;

	fn neg(self) -> Self::Output {
		Vex(self.0.map(Neg::neg))
	}
}

// Mul:

impl<const N: usize, A, B, T> Mul<A> for Vex<N, T>
where
	T: Mul<A, Output = B>,
	A: Clone,
{
	type Output = Vex<N, B>;

	fn mul(self, rhs: A) -> Self::Output {
		Vex(self.0.map(|x| x * rhs.clone()))
	}
}

impl<'a, const N: usize, A, B, T> Mul<A> for &'a Vex<N, T>
where
	&'a T: Mul<A, Output = B>,
	A: Clone,
{
	type Output = Vex<N, B>;

	fn mul(self, rhs: A) -> Self::Output {
		Vex(map_r(&self.0, |x| x * rhs.clone()))
	}
}

// Div:

impl<const N: usize, A, B, T> Div<A> for Vex<N, T>
where
	T: Div<A, Output = B>,
	A: Clone,
{
	type Output = Vex<N, B>;

	fn div(self, rhs: A) -> Self::Output {
		Vex(self.0.map(|x| x / rhs.clone()))
	}
}

impl<'a, const N: usize, A, B, T> Div<A> for &'a Vex<N, T>
where
	&'a T: Div<A, Output = B>,
	A: Clone,
{
	type Output = Vex<N, B>;

	fn div(self, rhs: A) -> Self::Output {
		Vex(map_r(&self.0, |x| x / rhs.clone()))
	}
}

impl<const N: usize, T: Zero> Zero for Vex<N, T> {
	const ZERO: Self = Self([T::ZERO; N]);
}

// Conveniences for scaling and zooming for vectors.
impl<const N: usize> Vex<N, Vx> {
	pub fn z(self, zoom: Zoom) -> Vex<N, Lx> {
		self.map(|x| x.z(zoom))
	}
}

impl<const N: usize> Vex<N, Lx> {
	pub fn z(self, zoom: Zoom) -> Vex<N, Vx> {
		self.map(|x| x.z(zoom))
	}

	pub fn s(self, scale: Scale) -> Vex<N, Px> {
		self.map(|x| x.s(scale))
	}
}

impl<const N: usize> Vex<N, Px> {
	pub fn s(self, scale: Scale) -> Vex<N, Lx> {
		self.map(|x| x.s(scale))
	}
}
