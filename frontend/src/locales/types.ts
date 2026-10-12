import type { en } from './en'

export interface LocaleResource {
  [key: string]: string | LocaleResource
}

export const PLURAL_SUFFIXES = ['zero', 'one', 'two', 'few', 'many', 'other'] as const
type PluralSuffix = (typeof PLURAL_SUFFIXES)[number]
type StripPlural<K extends string> = K extends `${infer Base}_${PluralSuffix}` ? Base : K

type Leaves<T, Prefix extends string = ''> = {
  [K in keyof T & string]: T[K] extends string ? StripPlural<`${Prefix}${K}`> : Leaves<T[K], `${Prefix}${K}.`>
}[keyof T & string]

/** Every UI string key, with plural suffixes folded into the base key (`deck.cards` + `{ count }`). */
export type TranslationKey = Leaves<typeof en>
