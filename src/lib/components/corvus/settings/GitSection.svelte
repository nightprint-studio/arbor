<script lang="ts">
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import GitProviderList from '$lib/components/shared/internal/GitProviderList.svelte';
</script>

<SectionHeader title="Git" description="Connect to Git hosting providers. Credentials are stored in the OS keychain." />

<!-- The provider cards are shared with the cross-product Credentials modal, so
     the two surfaces can never drift apart. -->
<GitProviderList />
