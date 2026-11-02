import { error } from '@sveltejs/kit';
import { dev } from '$app/environment';

// The gallery renders every modal with fixtures; it has no business in a shipped build.
export const load = () => {
  if (!dev) error(404, 'Not found');
};
